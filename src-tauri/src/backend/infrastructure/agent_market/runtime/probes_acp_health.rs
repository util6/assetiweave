use sha2::{Digest, Sha256};

use crate::backend::{
    domain::agents::{AgentInstallation, InstallationStatus, ProtocolStatus, RuntimeStatus},
    infrastructure::agent_execution::{
        backends::acp::{
            AcpConnectionStage, AcpModelDiscoveryOutcome, AcpProbeReport,
            AcpProtocolConnectionOutcome,
        },
        AgentConnectionResult,
    },
};

use super::{model_cache::AgentProbeIdentity, AgentRuntimeManager};

impl AgentRuntimeManager {
    pub(super) async fn persist_acp_probe_health(
        &self,
        agent_id: &str,
        expected_identity: &AgentProbeIdentity,
        report: &AcpProbeReport,
    ) {
        if matches!(
            report.protocol_connection,
            AcpProtocolConnectionOutcome::Cancelled
        ) {
            return;
        }

        let mut installation = match self.repository.get(agent_id).await {
            Ok(Some(inst)) => inst,
            _ => return,
        };

        let current_identity = probe_identity(&installation);
        if current_identity != *expected_identity {
            tracing::warn!(
                agent_id,
                "ACP probe finished but installation identity changed; skipped health persistence"
            );
            return;
        }

        let now = chrono::Utc::now().to_rfc3339();
        installation.runtime_checked_at = Some(now.clone());
        installation.protocol_checked_at = Some(now.clone());
        installation.model_checked_at = Some(now.clone());
        installation.updated_at = now.clone();

        match &report.protocol_connection {
            AcpProtocolConnectionOutcome::Connected => {
                installation.installation_status = InstallationStatus::Ready;
                installation.runtime_status = RuntimeStatus::Ready;
                installation.runtime_error_code = None;
                installation.runtime_error_message = None;
                installation.protocol_status = ProtocolStatus::Ready;
                installation.protocol_error_code = None;
                installation.protocol_error_message = None;

                match &report.model_discovery {
                    AcpModelDiscoveryOutcome::Success { .. } => {
                        installation.model_status = Some("ready".to_string());
                        installation.model_error_code = None;
                    }
                    AcpModelDiscoveryOutcome::Empty => {
                        installation.model_status = Some("unsupported".to_string());
                        installation.model_error_code = Some("model_list_empty".to_string());
                    }
                    AcpModelDiscoveryOutcome::Invalid { error_code, .. } => {
                        installation.model_status = Some("failed".to_string());
                        installation.model_error_code = Some(error_code.clone());
                    }
                    AcpModelDiscoveryOutcome::Timeout => {
                        installation.model_status = Some("failed".to_string());
                        installation.model_error_code = Some("model_discovery_timeout".to_string());
                    }
                    AcpModelDiscoveryOutcome::Unsupported => {
                        installation.model_status = Some("unsupported".to_string());
                        installation.model_error_code = Some("unsupported".to_string());
                    }
                    AcpModelDiscoveryOutcome::Failed { error_code, .. } => {
                        installation.model_status = Some("failed".to_string());
                        installation.model_error_code = Some(error_code.clone());
                    }
                    AcpModelDiscoveryOutcome::Skipped => {}
                }
            }
            AcpProtocolConnectionOutcome::Failed {
                stage,
                error_code,
                error_message,
            } => {
                if matches!(stage, AcpConnectionStage::Spawn) {
                    installation.installation_status = InstallationStatus::Broken;
                    installation.runtime_status = RuntimeStatus::Failed;
                    installation.runtime_error_code = Some(error_code.clone());
                    installation.runtime_error_message = Some(error_message.clone());
                }
                if error_code == "auth_required" {
                    installation.protocol_status = ProtocolStatus::AuthRequired;
                } else {
                    installation.protocol_status = ProtocolStatus::Failed;
                }
                installation.protocol_error_code = Some(error_code.clone());
                installation.protocol_error_message = Some(error_message.clone());
                installation.model_status = Some("failed".to_string());
                installation.model_error_code = Some(error_code.clone());
            }
            AcpProtocolConnectionOutcome::Cancelled => {}
        }

        if let Err(error) = self.repository.update_health(&installation).await {
            tracing::error!(
                agent_id,
                error = %error,
                "Failed to persist ACP health to repository"
            );
        }
    }
}

pub(crate) fn unavailable_acp_connection(
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
        connection_method: Some("acp".to_string()),
        error_code: Some(code.to_string()),
        error: Some(message.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

pub(crate) fn probe_identity(installation: &AgentInstallation) -> AgentProbeIdentity {
    let mut digest = Sha256::new();
    digest.update(installation.agent_id.as_bytes());
    digest.update([0]);
    digest.update(installation.installation_id.as_bytes());
    digest.update([0]);
    digest.update(installation.catalog_item_version.as_bytes());
    digest.update([0]);
    digest.update(installation.agent_version.as_bytes());
    digest.update([0]);
    digest.update(installation.distribution_id.as_bytes());
    digest.update([0]);
    digest.update(installation.definition_json.to_string().as_bytes());
    digest.update([0]);
    digest.update(installation.resolved_program.to_string_lossy().as_bytes());
    digest.update([0]);
    for arg in &installation.args {
        digest.update(arg.as_bytes());
        digest.update([0]);
    }
    digest.update([installation.enabled as u8]);
    digest.update([installation.resolved_program.is_file() as u8]);
    AgentProbeIdentity {
        agent_id: installation.agent_id.clone(),
        installation_id: installation.installation_id.clone(),
        definition_digest: format!("{:x}", digest.finalize()),
        enabled: installation.enabled,
        executable_present: installation.resolved_program.is_file(),
    }
}
